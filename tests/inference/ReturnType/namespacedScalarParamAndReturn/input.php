<?php
namespace Foo;

/**
 * @param scalar $scalar
 *
 * @return scalar
 */
function foo($scalar) {
    switch(random_int(0, 3)) {
        case 0:
            return true;
        case 1:
            return "string";
        case 2:
            return 2;
        case 3:
            return 3.0;
    }

    return 0;
}
