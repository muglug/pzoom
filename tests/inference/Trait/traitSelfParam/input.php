<?php
trait T {
    public function bar(self $object): self {
        return $this;
    }
}

class Foo {
    use T;
}

$f1 = new Foo();
$f2 = (new Foo())->bar($f1);
