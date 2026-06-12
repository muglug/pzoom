<?php
/**
 * @return "a"|"b"|null
 */
function foo() : ?string {
    if (rand(0, 1)) {
        return "a";
    }

    if (rand(0, 1)) {
        return "b";
    }
}
