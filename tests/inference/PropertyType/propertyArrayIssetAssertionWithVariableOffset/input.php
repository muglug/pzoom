<?php
function bar(string $s): void { }

class A {
    /** @var array<string, string> */
    public $a = [];

    private function foo(): void {
        $b = "hello";

        if (!isset($this->a[$b])) {
            return;
        }

        bar($this->a[$b]);
    }
}
