<?php
/** @param stringable-object $o */
function acceptsStringableObject(object $o): void {}

class C {}
acceptsStringableObject(new C);
                
