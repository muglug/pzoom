<?php
final class C {
    public function foo() : void {
        if (rand(0, 1)) {
            $this->foo();
        }
    }

    public function bar() : void {}
}

(new C)->bar();
