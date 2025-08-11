1. DiffExecutor对executor没有限制（可以使用Forkserver）
   - [ ] ForkServer疑似不支持细粒度return value（只有ok，crash，timeout）
2. app下的libpng和libspng已经修改为适应rust的模式，rust编译模式里更推崇使用Harness+静态编译链接（可参阅inprocess下的libpng的编译）
   - [ ] 需要验证之前编译模式在rust下的适应性
   - [ ] 需要确认是否需要将依赖的库进行自主编译（如libpng的依赖libzlib等）