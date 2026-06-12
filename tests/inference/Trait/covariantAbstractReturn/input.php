<?php
trait T {
    /** @return iterable */
    abstract public function bar();
}

class C {
    use T;

    /** @return array */
    public function bar() { return []; }
}
