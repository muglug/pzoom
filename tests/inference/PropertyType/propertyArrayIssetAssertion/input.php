<?php
function bar(string $s): void { }

class A {
    /** @var array<string, string> */
    public $a = [];

    private function foo(): void {
        if (isset($this->a["hello"])) {
            bar($this->a["hello"]);
        }
    }
}
