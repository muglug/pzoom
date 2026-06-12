<?php
final class A {
    public string $foo = "bar";
    public array $bar = [];
}

$a = new A();
$a->bar[$a->foo] = "bar";
print_r($a->bar);
