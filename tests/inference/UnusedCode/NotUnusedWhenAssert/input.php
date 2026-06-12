<?php

final class A {
    public function getVal(?string $val): string {
        $this->assert($val);

        return $val;
    }

    /**
     * @psalm-assert string $val
     * @psalm-mutation-free
     */
    private function assert(?string $val): void {
        if (null === $val) {
            throw new Exception();
        }
    }
}

$a = new A();
echo $a->getVal(null);
