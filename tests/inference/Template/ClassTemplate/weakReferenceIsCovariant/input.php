<?php
/** @param WeakReference<Throwable> $_ref */
function acceptsThrowableRef(WeakReference $_ref): void {}

acceptsThrowableRef(WeakReference::create(new Exception));
                