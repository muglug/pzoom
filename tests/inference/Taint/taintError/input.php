<?php
function foo() {}
try {
    foo();
} catch (TypeError $e) {
    echo "Caught: {$e->getTraceAsString()}\n";
}
