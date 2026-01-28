<?php
class A {}
class B {}

/**
 * @return array<class-string>
 */
function takesClassConstants() : array {
    return ["A", "B"];
}
