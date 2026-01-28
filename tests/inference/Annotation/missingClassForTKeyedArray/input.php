<?php
interface I {
    /** @return object{id: int, a: int} */
    public function run();
}

class C implements I {
    /** @return X */
    public function run() {}
}
