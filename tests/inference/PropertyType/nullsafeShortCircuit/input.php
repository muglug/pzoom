<?php
class Foo {
    private ?self $nullableSelf = null;

    public function __construct(private self $self) {}

    public function doBar(): ?self
    {
        return $this->nullableSelf?->self->self;
    }
}
