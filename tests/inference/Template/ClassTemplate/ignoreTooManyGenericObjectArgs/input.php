<?php
/**
 * @template T
 */
class C {
    /** @var T */
    public $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }
}

/** @param C<int> $c */
function takesC(C $c) : void {}

/**
 * @psalm-suppress TooManyTemplateParams
 * @var C<int, int>
 */
$c = new C(5);
takesC($c);