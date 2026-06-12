<?php
interface I {}
class C implements I {}
$c_instance = new C;

class Props {
    /** @var class-string<I>[] */
    public $arr = [];
}

(new Props)->arr[] = get_class($c_instance);
