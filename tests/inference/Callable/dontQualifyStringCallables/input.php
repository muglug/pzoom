<?php
namespace NS;

function ff() : void {}

function run(callable $f) : void {
    $f();
}

run("ff");
