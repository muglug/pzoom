<?php
class C {
    public function __invoke(): string {
        return "You ran?";
    }
}

function foo(callable $c): void {
    echo (string)$c();
}

foo(new C());

$c2 = new C();
$c2();
