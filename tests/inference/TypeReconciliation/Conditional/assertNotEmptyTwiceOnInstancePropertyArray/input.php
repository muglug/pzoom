<?php
class A {
    private array $c = [];

    public function bar(string $s, string $t): void {
        if (empty($this->c[$s]) && empty($this->c[$t])) {}
    }
}