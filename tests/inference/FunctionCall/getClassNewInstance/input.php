<?php
interface I {}
class C implements I {}

class Props {
    /** @var class-string<I>[] */
    public $arr = [];
}

(new Props)->arr[] = get_class(new C);
