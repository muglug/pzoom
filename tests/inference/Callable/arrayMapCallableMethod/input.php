<?php
class A {
    public static function bar(string $a): string {
        return $a . "b";
    }
}

function baz(string $a): string {
    return $a . "b";
}

$a = array_map("A::bar", ["one", "two"]);
$b = array_map(["A", "bar"], ["one", "two"]);
$c = array_map([A::class, "bar"], ["one", "two"]);
$d = array_map([new A(), "bar"], ["one", "two"]);
$a_instance = new A();
$e = array_map([$a_instance, "bar"], ["one", "two"]);
$f = array_map("baz", ["one", "two"]);
