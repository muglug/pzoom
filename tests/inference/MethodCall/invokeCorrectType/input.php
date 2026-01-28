<?php
class A {
    public function __invoke(string $p): void {}
}

$q = new A;
$q("asda");
