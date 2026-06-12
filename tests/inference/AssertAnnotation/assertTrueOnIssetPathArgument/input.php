<?php
class FuncDocblock3 {
    /**
     * @var array<
     *     int,
     *     array{
     *         name:string,
     *         description?: string
     *     }
     * >
     */
    public array $params = [];
}

abstract class TestBase {
    /**
     * @psalm-assert true $condition
     */
    public function assertTrue(bool $condition, string $message = ''): void {}
    /**
     * @psalm-assert =T $expected
     * @template T
     * @param T $expected
     */
    public function assertSame($expected, mixed $actual, string $message = ''): void {}
}

class MyTest extends TestBase {
    public function testIt(FuncDocblock3 $function_docblock): void {
        $this->assertTrue(isset($function_docblock->params[0]['description']));
        $this->assertSame('x', $function_docblock->params[0]['description']);

        $this->assertTrue(isset($function_docblock->params[1]['description']));
        $this->assertSame('y', $function_docblock->params[1]['description']);
    }
}
