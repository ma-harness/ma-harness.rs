// ma-harness Go streaming gRPC client example (P5-7 / Day 96).
//
// 跑:
//   1. go mod tidy
//   2. ./compile_proto.sh
//   3. mah start --grpc-port 50051 --http-port 50050
//   4. go run stream_client.go
//
// 演示:
// - 调 AgentService.RunStream RPC
// - 走 gRPC server-streaming, Go 端 stub.RunStream(req) 返 server stream
// - 业务方 for { stream.Recv() } 拿 AgentStreamEvent
// - io.EOF 标志 stream 结束
//
// 跟 example_client.go 区别:
// - 同步 Run 走 stub.Run(ctx, req) 拿 single response
// - 异步 RunStream 走 stub.RunStream(ctx, req) 返 ServerStream

package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"time"

	pb "github.com/ma-harness/ma-harness-client/ma_harness_pb/ma_harness/v1"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

const (
	serverAddr = "localhost:50051"
)

func runStream(ctx context.Context, stub pb.AgentServiceClient, sessionID, message string) ([]string, error) {
	req := &pb.AgentRunRequest{
		SessionId: sessionID,
		Input: &pb.Message{
			Role: pb.ToolRole_TOOL_ROLE_USER,
			Content: []*pb.ContentBlock{
				{Content: &pb.ContentBlock_Text{Text: &pb.TextBlock{Text: message}}},
			},
		},
		ModelConfig: &pb.ModelConfig{
			Adapter:    pb.ModelAdapter_MODEL_ADAPTER_STUB,
			Model:      "stub",
			Temperature: 0,
			MaxTokens:   100,
		},
	}

	// RunStream 返 *AgentService_RunStreamClient (server stream)
	stream, err := stub.RunStream(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("RunStream: %w", err)
	}

	var tokens []string
	for {
		event, err := stream.Recv()
		if err == io.EOF {
			// stream 结束
			break
		}
		if err != nil {
			return nil, fmt.Errorf("Recv: %w", err)
		}
		// event.Event 是 oneof, 走 type switch
		switch e := event.Event.(type) {
		case *pb.AgentStreamEvent_Message:
			// 拿第一个 text content
			if msg := e.Message; msg != nil && len(msg.Content) > 0 {
				if textBlock, ok := msg.Content[0].Content.(*pb.ContentBlock_Text); ok {
					token := textBlock.Text.Text
					tokens = append(tokens, token)
					fmt.Printf("  [token] %q\n", token)
				}
			}
		}
	}
	return tokens, nil
}

func main() {
	conn, err := grpc.Dial(serverAddr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
		grpc.WithTimeout(5*time.Second),
	)
	if err != nil {
		log.Fatalf("dial %s: %v", serverAddr, err)
	}
	defer conn.Close()

	stub := pb.NewAgentServiceClient(conn)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	fmt.Println("=== RunStream (3-word message) ===")
	tokens, err := runStream(ctx, stub, "stream-demo", "alpha beta gamma")
	if err != nil {
		log.Fatalf("runStream: %v", err)
	}

	full := ""
	for _, t := range tokens {
		full += t
	}
	fmt.Printf("\n=== Done ===\n")
	fmt.Printf("  total events: %d\n", len(tokens))
	fmt.Printf("  full content: %q\n", full)
	if len(tokens) != 3 {
		log.Fatalf("FAIL: expected 3 events, got %d", len(tokens))
	}
	if full != "alpha beta gamma " {
		log.Fatalf("FAIL: expected 'alpha beta gamma ', got %q", full)
	}
	fmt.Println("OK: 3 word events 拼回 'alpha beta gamma '")
}
