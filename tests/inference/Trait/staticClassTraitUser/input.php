<?php
trait T {
    public function f(): void {
        if (static::class === B::class) {
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
