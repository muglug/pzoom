<?php
function foo() : Traversable {
    if (rand(0, 1)) {
        yield "hello";
        return;
    }

    yield "goodbye";
}
