<?php
final class A {
    /** @var list<string> */
    public array $foo = [];
}

$a = new A();
$a->foo[] = "bar";
