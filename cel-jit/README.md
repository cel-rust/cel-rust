# cel-jit

This project is an experiment to evaluate the performance benefits of JIT compilation for simple, embedded expression languages like CEL.

Expression languages such as CEL and VRL look simple, but their primary use cases are in very high-throughput environments. I have wondered whether any compile-time optimization is worth paying for in those languages as well.

I was not sure because:

- Its AST is pretty simple, so a tree-walk interpreter is already sufficiently fast.
- Complex types and operations, such as regular expression processing, cannot be inlined and therefore won't be optimized.
- Implementing and maintaining JIT compilation is a very complex task. Without advanced optimization techniques, there's no guarantee it will be faster.

But now it's 2025, and it's a good time to try. We have Cranelift and Claude Code. I asked Opus 4.5 to implement a JIT compiler for CEL using Cranelift, and it did a great job.

You can see the detail from [EXPERIMENT_LOG](./EXPERIMENT_LOG.md) written by Claude Code.

Here's my concluusion:

- It's worth having. It offers significant improvements for simple arithmetic and logic operations.
- However, in real-world use cases, the more complex context object or host functions like regular expressions, the less efficient it becomes.
- Compilation is expensive, but in most cases it can be compiled statically, ahead-of-time, and stored as CLIF format. Do not use it for dynamic expressions.
- Use cases matter. While this may be useful in CEL, where the policy engine is the primary use case, I would expect it to be worthless in VRL, where text parsing and transformation are the primary use cases.
- Do not allow `unsafe` to coding agents. Or they will make a ton of memory leaks.
