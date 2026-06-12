<?php
interface I1 {
    public function test(string $s) : ?string;
    public function testIterable(array $a) : ?iterable;
}

class A1 implements I1 {
    public function test(?string $s) : string {
        return "value";
    }
    public function testIterable(?iterable $a) : array {
        return [];
    }
}
