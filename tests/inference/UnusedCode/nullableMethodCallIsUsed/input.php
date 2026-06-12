<?php
final class Test {
    public function test(): void {
    }
}

final class TestFactory {
    /**
     * @psalm-pure
     */
    public function create(bool $returnNull): ?Test {
        if ($returnNull) {
            return null;
        }

        return new Test();
    }
}

$factory = new TestFactory();
$factory->create(false)?->test();

$exception = new \Exception();

throw ($exception->getPrevious() ?? $exception);
