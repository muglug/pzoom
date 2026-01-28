<?php
                    namespace A
                    {
                        class Foo
                        {
                            /**
                             * @psalm-internal \B
                             * @psalm-internal \C
                             */
                            public static function foobar(): void {}
                        }
                    }

                    namespace B
                    {
                        \A\Foo::foobar();
                    }

                    namespace C
                    {
                        \A\Foo::foobar();
                    }
