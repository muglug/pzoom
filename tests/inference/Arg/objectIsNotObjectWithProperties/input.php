<?php

function makeObj(): object {
    return (object)["a" => 42];
}

/** @param object{hmm:float} $_o */
function takesObject($_o): void {}

takesObject(makeObj()); // expected: ArgumentTypeCoercion
                
