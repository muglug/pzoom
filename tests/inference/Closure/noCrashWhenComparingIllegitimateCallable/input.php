<?php
class C {}

function foo() : C {
    return fn (int $i) => "";
}
