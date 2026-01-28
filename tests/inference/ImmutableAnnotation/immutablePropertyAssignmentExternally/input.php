<?php
/**
 * @psalm-immutable
 */
class A {
    /** @var int */
    private $a;

    /** @var string */
    public $b;

    public function __construct(int $a, string $b) {
        $this->a = $a;
        $this->b = $b;
    }
}

$a = new A(4, "hello");

$a->b = "goodbye";
