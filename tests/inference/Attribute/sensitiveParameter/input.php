<?php

namespace SensitiveParameter;

use SensitiveParameter;

class HelloWorld {
    public function __construct(
        #[SensitiveParameter] string $password
    ) {}
}
                
