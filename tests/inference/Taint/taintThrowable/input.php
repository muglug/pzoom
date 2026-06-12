<?php
function foo() {}
try {
    foo();
} catch (Throwable $e) {
    echo "Caught: $e";  // TODO: ("Caught" . $e) does not work.
}
