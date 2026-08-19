// ma-harness Go gRPC client example.
//
// 跑:
//   1. go mod tidy
//   2. ./compile_proto.sh
//   3. mah start --grpc-port 50051 --http-port 50050
//   4. go run example_client.go
//
// 演示:
// - ListSessions: 列出所有 session
// - CreateSession: 创建一个新 session
// - RunAgent: 跑一次 agent (本地 stub model, 不真 LLM)
// - GetSessionEvents: 拿 session 事件
package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"time"

	pb "github.com/ma-harness/ma-harness-client/ma_harness_pb/ma_harness/v1"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

const (
	serverAddr = "localhost:50051"
	dialTimeout = 5 * time.Second
)

func listSessions(ctx context.Context, stub pb.SessionServiceClient) ([]*pb.Session, error) {
	resp, err := stub.ListSessions(ctx, &pb.ListSessionsRequest{
		Page:     1,
		PageSize: 10,
	})
	if err != nil {
		return nil, fmt.Errorf("ListSessions: %w", err)
	}
	return resp.Sessions, nil
}

func createSession(ctx context.Context, stub pb.SessionServiceClient, name string) (string, error) {
	resp, err := stub.CreateSession(ctx, &pb.CreateSessionRequest{
		Name:          name,
		OperatingMode: pb.OperatingMode_OPERATING_MODE_DEFAULT,
	})
	if err != nil {
		return "", fmt.Errorf("CreateSession: %w", err)
	}
	return resp.Session.Id, nil
}

func runAgent(ctx context.Context, stub pb.AgentServiceClient, sessionID, message string) (string, error) {
	resp, err := stub.Run(ctx, &pb.RunRequest{
		SessionId:   sessionID,
		UserMessage: message,
		Model:       "stub",
		Temperature: 0.7,
		MaxTokens:   1024,
	})
	if err != nil {
		return "", fmt.Errorf("Run: %w", err)
	}
	return resp.ModelResponse.Content, nil
}

func getSessionEvents(ctx context.Context, stub pb.SessionServiceClient, sessionID string) ([]*pb.SessionEvent, error) {
	resp, err := stub.GetSessionEvents(ctx, &pb.GetSessionEventsRequest{
		SessionId: sessionID,
		Limit:     20,
	})
	if err != nil {
		return nil, fmt.Errorf("GetSessionEvents: %w", err)
	}
	return resp.Events, nil
}

func main() {
	// 1. 连 gRPC server (insecure for dev, prod 用 TLS credentials)
	conn, err := grpc.Dial(serverAddr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
		grpc.WithTimeout(dialTimeout),
	)
	if err != nil {
		log.Fatalf("dial %s: %v", serverAddr, err)
	}
	defer conn.Close()

	agentStub := pb.NewAgentServiceClient(conn)
	sessionStub := pb.NewSessionServiceClient(conn)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	// 2. 列出现有 session
	fmt.Println("=== Existing sessions ===")
	sessions, err := listSessions(ctx, sessionStub)
	if err != nil {
		log.Fatalf("listSessions: %v", err)
	}
	for _, s := range sessions {
		fmt.Printf("  %s... name=%q state=%v\n", s.Id[:8], s.Name, s.State)
	}

	// 3. 创建一个新 session
	fmt.Println("\n=== Creating new session ===")
	newID, err := createSession(ctx, sessionStub, "go-example")
	if err != nil {
		log.Fatalf("createSession: %v", err)
	}
	fmt.Printf("  new session id = %s\n", newID)

	// 4. 跑一次 agent (stub model, 不发真 LLM)
	fmt.Println("\n=== Running agent (stub model) ===")
	content, err := runAgent(ctx, agentStub, newID, "hello from Go")
	if err != nil {
		log.Fatalf("runAgent: %v", err)
	}
	fmt.Printf("  response: %q\n", content)

	// 5. 拿 events
	fmt.Println("\n=== Session events ===")
	events, err := getSessionEvents(ctx, sessionStub, newID)
	if err != nil {
		log.Fatalf("getSessionEvents: %v", err)
	}
	for i, e := range events {
		if i >= 5 {
			fmt.Printf("  ... and %d more\n", len(events)-5)
			break
		}
		fmt.Printf("  seq=%d type=%v severity=%v\n", e.Seq, e.EventType, e.Severity)
	}

	os.Exit(0)
}
