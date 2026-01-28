<?php
class A {}

/**
 * @return array<class-string>
 */
function takesClassConstants() : array {
    return ["A", "B"];
}
