<?php
class A {
    protected function getC() : C {
        return new C;
    }
}
final class C {
    public function foo() : void {}
}

final class B extends A {
    public function bar() : void {
        $c = $this->getC();

        foreach ([1, 2, 3] as $_) {
            try {
                $c->foo();
            } catch (Exception $e) {}
        }
    }
}

(new B)->bar();
