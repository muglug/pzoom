<?php
function makeObj(): object {
    return (object)["a" => 42];
}

/** @return object{hmm:float} */
function f(): object {
    return makeObj();
}
