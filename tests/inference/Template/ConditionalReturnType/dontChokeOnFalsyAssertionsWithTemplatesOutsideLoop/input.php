<?php
class A {}
class B {}

/**
 * @psalm-return ($a is true ? list<A> : list<B>)
 */
function get_liste_designation_client(bool $a = false) {
    (!$a ? "a" : "b");
    (!$a ? "a" : "b");
    return [];
}