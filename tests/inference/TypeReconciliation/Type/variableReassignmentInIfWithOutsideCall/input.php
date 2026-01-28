<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class Two {
    /** @return void */
    public function barBar() {}
}

$one = new One();

if (1 + 1 === 2) {
    $one = new Two();

    $one->barBar();
}

$one->barBar();
