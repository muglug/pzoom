<?php
/**
 * @mixin B
 */
class A {
    /** @var B */
    private $b;

    public function __construct() {
        $this->b = new B();
    }

    public function c(string $s) : void {}

    /**
     * @param array<mixed> $arguments
     * @return mixed
     */
    public function __call(string $method, array $arguments)
    {
        return $this->b->$method(...$arguments);
    }
}

class B {
    public function b(): void {
        echo "b";
    }

    public function c(int $s) : void {}
}

$a = new A();
$a->b();
