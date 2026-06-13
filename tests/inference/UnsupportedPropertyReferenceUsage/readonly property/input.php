<?php
class A {
    public function __construct(
        public readonly int $b,
    ) {
    }
}
$a = new A(0);
$b = &$a->b;
$b = 1; // Fatal error
