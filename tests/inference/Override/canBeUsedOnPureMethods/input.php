<?php
class A {
    /** @psalm-pure */
    public function f(): void {}
}
class B extends A {
    /** @psalm-pure */
    #[Override]
    public function f(): void {}
}
