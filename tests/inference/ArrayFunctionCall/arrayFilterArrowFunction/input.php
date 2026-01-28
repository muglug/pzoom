<?php
class A {}
class B {}

$a = \array_filter(
    [new A(), new B()],
    function($x) {
        return $x instanceof B;
    }
);

$b = \array_filter(
    [new A(), new B()],
    fn($x) => $x instanceof B
);
