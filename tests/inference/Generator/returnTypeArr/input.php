<?php
function foo() : Generator {
    $result = yield from [2];
    /** @psalm-check-type-exact $result = null */;
}
function foo2() : Generator {
    $result = yield from [];
    /** @psalm-check-type-exact $result = null */;
}
