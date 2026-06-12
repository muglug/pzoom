<?php
interface I {}

class Props {
    /** @var class-string<I>[] */
    public $arr = [];
}

(new Props)->arr[] = get_class(new class implements I{});
