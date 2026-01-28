<?php
/**
 * @psalm-immutable
 */
class A {
    /** @var string */
    public $b;

    public function __construct(string $b) {
        $this->b = $b;
    }
}

$a = new A("hello");
unset($a->b);
