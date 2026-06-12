<?php

class C {
    protected int $foo = 1;
    public function bar() : void {
        $this->foo = 5;
    }

    public function getFoo(): void {
        echo $this->foo;
    }
}

final class D extends C {
    protected int $foo = 2;
}

(new D)->bar();
(new D)->getFoo();
