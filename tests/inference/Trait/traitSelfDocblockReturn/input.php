<?php
trait T {
    /** @return self */
    public function getSelf() {
        return $this;
    }
}

class C {
    use T;
}
