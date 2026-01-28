<?php
                    namespace A
                    {
                        class Foo
                        {
                            /** @psalm-internal B\Bar */
                            public static function foo(): void {}

                            /** @psalm-internal B\Bar */
                            public function bar(): void {}
                        }
                    }

                    namespace B
                    {
                        class Bar
                        {
                            public function baz(): void
                            {
                                \A\Foo::foo();
                                $foo = new \A\Foo();
                                $foo->bar();
                            }
                        }
                    }
