<?php
trait SomeTrait {
    abstract public function a(self $b): self;
}

class SomeClass {
    use SomeTrait;

    public function a(self $b): self {
        return $this;
    }
}
