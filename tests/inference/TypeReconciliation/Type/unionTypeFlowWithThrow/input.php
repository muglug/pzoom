<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

/** @return void */
function a(One $var = null) {
    if (!$var) {
        throw new \Exception("some exception");
    }
    else {
        $var->fooFoo();
    }
}