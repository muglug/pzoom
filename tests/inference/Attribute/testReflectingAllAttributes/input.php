<?php
/** @var class-string $a */
$cs = stdClass::class;

$a = new ReflectionClass($cs);
$b = $a->getAttributes();
                    
