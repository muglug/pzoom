<?php
trait T {
    public function g(): self
    {
        return $this;
    }
}

class A {
    use T;
}

class B extends A {
}

class C {
    use T;
}

$a = (new B)->g();
