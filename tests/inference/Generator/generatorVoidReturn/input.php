<?php
/**
 * @return Generator
 */
function generator2() : Generator {
    if (rand(0,1)) {
        return;
    }
    yield 2;
}
