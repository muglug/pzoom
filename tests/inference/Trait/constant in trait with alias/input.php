<?php
trait TraitA { private const PRIVATE_CONST = 'PRIVATE_CONST'; }
class ClassB { use TraitA { PRIVATE_CONST as public PUBLIC_CONST; } }
$c = ClassB::PUBLIC_CONST;
