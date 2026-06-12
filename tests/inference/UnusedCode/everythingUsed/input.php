<?php
interface I {
    public function foo() : void;
}
final class B implements I {
    public function foo() : void {}
}

final class A
{
    /**
     * @var I
     */
    private $i;

    /**
     * @param int[] $as
     */
    public function __construct(array $as) {
        $this->i = new B();

        foreach ($as as $a) {
            $this->a($a, 1);
        }
    }

    private function a(int $a, int $b): void
    {
        $this->v($a, $b);

        $this->i->foo();
    }

    private function v(int $a, int $b): void
    {
        if ($a + $b > 0) {
            throw new \RuntimeException("");
        }
    }
}

new A([1, 2, 3]);
