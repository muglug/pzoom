<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

/** @var One|null */
$var = null;

if (rand(0,100) === 5) {

}
elseif (!$var) {

}
else {
    $var->fooFoo();
}