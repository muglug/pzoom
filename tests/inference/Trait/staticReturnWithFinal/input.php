<?php
trait T {
    /** @return static */
    public function instance() {
        return new static();
    }
}

final class A {
    use T;
}
