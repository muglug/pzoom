<?php
$e = new Exception;
$r = WeakReference::create($e);
$ex = $r->get();
                