<?php
use ImplementationRequirements\Base\A;
use ImplementationRequirements\Base\B;
use ImplementationRequirements\Trait\ImposesImplementationRequirements;

class Valid implements A, B {
    use ImposesImplementationRequirements;
}
                