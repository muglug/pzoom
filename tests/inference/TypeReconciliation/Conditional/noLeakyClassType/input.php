<?php
class A {
    public array $foo = [];
    public array $bar = [];

    public function setter() : void {
        if ($this->foo) {
            $this->foo = [];
        }
    }

    public function iffer() : bool {
        return $this->foo || $this->bar;
    }
}