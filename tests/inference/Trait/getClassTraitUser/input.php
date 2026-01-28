<?php
trait T {
    public function f(): void {
        if (get_class($this) === B::class) {
            $this->foo();
        }
    }
}

class A {
    use T;
}

class B {
    use T;

    public function foo() : void {}
}
