<?php
class A {}

/**
 * @return class-string
 */
function takesClassConstants() : string {
    return A::class;
}
