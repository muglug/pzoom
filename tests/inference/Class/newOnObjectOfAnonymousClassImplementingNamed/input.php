<?php
interface I {}
function f(): I {
    $o = new class implements I {};
    return new $o;
}
