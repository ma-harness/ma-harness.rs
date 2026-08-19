;; examples/hello.wat — T3.1 Code Mode demo
;;
;; 编译并跑:
;;   mah code run examples/hello.wat
;;
;; 期望输出:
;;   --- stdout ---
;;   hello from wasm (T3.1)
;;   2 + 3 = 5
;;   --- return value: 5 ---

(module
    (import "host" "log" (func $log (param i32 i32)))

    (memory (export "memory") 1)

    ;; offset 0:   "hello from wasm (T3.1)" (22 chars)
    (data (i32.const 0) "hello from wasm (T3.1)")
    ;; offset 100: "2 + 3 = 5" (9 chars)
    (data (i32.const 100) "2 + 3 = 5")

    (func (export "run") (result i32)
        ;; 打印第一行
        i32.const 0
        i32.const 22
        call $log

        ;; 算 2 + 3
        i32.const 2
        i32.const 3
        i32.add
        ;; 现在 stack top = 5, drop 之后我们直接返 5
        ;; 但我们也打印 "2 + 3 = 5" (字面量, 不是计算后格式化的)
        i32.const 100
        i32.const 9
        call $log
        ;; drop 那个算出来的 5
        drop
        ;; 然后再算一次返 (避免上面 drop 后返 0)
        i32.const 2
        i32.const 3
        i32.add
    )
)
