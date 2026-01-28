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

$a = (new A)->g();
