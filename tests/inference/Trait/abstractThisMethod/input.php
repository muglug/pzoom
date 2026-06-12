<?php
trait ATrait {
    /** @return $this */
    abstract public function bar();
}

class C {
    use ATrait;

    /** @return $this */
    public function bar() {
        return $this;
    }
}
