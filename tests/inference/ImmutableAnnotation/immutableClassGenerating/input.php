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

    public function setA(int $a) : self {
        return new self($a, $this->b);
    }
}
