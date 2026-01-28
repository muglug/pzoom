<?php
/**
 * @template T
 */
class A {
    public function __construct(
        /** @var T */
        public mixed $t
    ) {}
}

$a = new A(5);
$t = $a->t;
                
