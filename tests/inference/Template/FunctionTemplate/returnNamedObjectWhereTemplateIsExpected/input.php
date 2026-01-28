<?php
class Bar {}

/**
 * @template T as object
 * @param T $t
 * @return T
 */
function shouldComplain(object $t) {
    return new Bar();
}
