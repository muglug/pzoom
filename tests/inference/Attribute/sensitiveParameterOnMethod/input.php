<?php

namespace SensitiveParameter;

use SensitiveParameter;

class HelloWorld {
    #[SensitiveParameter]
    public function __construct(
        string $password
    ) {}
}
                
