<?php
interface A {
}
interface B {
}
class Foo {
    public function __construct(private A&B $self) {}

    public function self(): A&B
    {
        return $this->self;
    }
}
