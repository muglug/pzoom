<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class Two {
    /** @return void */
    public function barBar() {}
}

class Three {
    /** @return void */
    public function baz() {}
}

/** @var One|Two|Three|null */
$var = null;

if ($var instanceof One) {
    $var->fooFoo();
}
else {
    if ($var instanceof Two) {
        $var->barBar();
    }
    else if ($var) {
        $var->baz();
    }
}