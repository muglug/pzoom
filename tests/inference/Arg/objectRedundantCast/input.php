<?php

function makeObj(): object {
    return (object)["a" => 42];
}

function takesObject(object $_o): void {}

takesObject((object)makeObj()); // expected: RedundantCast
                
