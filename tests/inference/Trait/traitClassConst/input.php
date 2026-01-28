<?php
trait A {
    public function foo(): string {
        return B::class;
    }
}

trait B {}

class C {
    use A;
}
