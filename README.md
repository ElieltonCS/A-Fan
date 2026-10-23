# A-Fan

> **PROJETO EXPERIMENTAL — CONTROLE FÍSICO DE FAN NÃO TESTADO NEM VALIDADO.**
>
> Desenvolvido especificamente para o **Acer Aspire A515-45 com AMD Ryzen 7 5700U, BIOS V1.58 e Windows 11 x64**. Existem testes de software e leituras de sensores, mas a redução real da rotação, a calibração e a recuperação física após falhas **não foram testadas nem validadas neste modelo**. Não há compatibilidade confirmada com outros notebooks. Os perfis de controle permanecem bloqueados quando o firmware não confirma suporte.

Aplicativo Windows x64 nativo em Rust/Win32 para o Acer Aspire A515-45. Abre oculto na bandeja; fechar ou minimizar a janela mantém o monitoramento. O menu **Sair** encerra após restaurar o controle Acer se o aplicativo o tiver alterado.

## Situação neste notebook

Diagnóstico elevado em 23/10/2026: Aspire A515-45, Ryzen 7 5700U, BIOS V1.58. Temperatura CPU e RPM funcionam via WMI. Os getters de modo e porcentagem da ventoinha retornam status **1**, indicando comando indisponível. A análise estática do pacote exato de BIOS V1.58 encontrou os métodos de controle independente implementados como retorno imediato de `EFI_UNSUPPORTED`. Não há um simples sinalizador de habilitação nesses métodos.

Por isso, **o A-Fan não altera a fan neste notebook**: monitoramento e prévias de presets funcionam; aplicar presets A-Fan e calibrar permanecem bloqueados. Nenhum setter foi executado na validação. A investigação do EC continua, mas ainda não há um protocolo independente validado. O controle de perfis usado pelo AcerSense também atualiza políticas térmicas/de potência da CPU e não serve como substituto para controle exclusivo da fan.

O aplicativo não modifica plano de energia, boost, limites CPU/GPU, perfis Acer nem BIOS. Não inclui escrita bruta no EC, drivers de acesso arbitrário a hardware, rede, telemetria externa ou atualização automática. A instalação preserva os componentes Acer existentes.

## Instalar

Este repositório publica o código-fonte; binários, diagnósticos e relatórios da máquina de desenvolvimento ficam fora do Git. Para gerar o pacote, é necessário Windows x64 com Rust estável, ferramentas de compilação C++ e Windows SDK instalados. Na raiz do projeto, execute no PowerShell:

```powershell
.\scripts\build.ps1
```

Depois, na pasta gerada `artifacts/release`, execute `install.ps1` com PowerShell. A instalação solicita administrador, copia o app para `%ProgramFiles%\A-Fan`, cria atalhos e registra a tarefa `A-Fan` para seu logon. Funciona na bateria e na tomada. O app aparece inicialmente só na bandeja; clique no ícone para abrir.

Na janela, **Iniciar com o Windows** habilita/desabilita somente o gatilho de logon. O atalho continua funcionando manualmente. A elevação é feita pela tarefa registrada, sem UAC a cada logon. O executável e os scripts da tarefa ficam em Program Files, protegidos contra edição por processos sem elevação.

Para remover, execute `%ProgramFiles%\A-Fan\uninstall.ps1`. Ele solicita saída segura, remove sua tarefa, seus atalhos e os arquivos conhecidos do app. Preserva as configurações e os diagnósticos em `%LOCALAPPDATA%\A-Fan`.

## Presets fixos

Não há ajuste manual de rotação, temperatura nem pontos da curva. A seleção exibe uma prévia e uma descrição; **Aplicar perfil** exige sensores, controle independente, processo de recuperação e calibração válidos. O menu da bandeja oferece os mesmos presets.

| Preset | 40°C | 55°C | 65°C | 75°C | 80°C ou mais |
| --- | ---: | ---: | ---: | ---: | ---: |
| Discreto | 40% | 40% | 55% | 85% | 100% |
| Auto | 40% | 50% | 65% | 90% | 100% |
| Resfriar | 50% | 65% | 80% | 100% | 100% |
| Carga intensa | 60% | 80% | 100% | 100% | 100% |
| Ventilação máxima | 100% | 100% | 100% | 100% | 100% |

Os percentuais representam comandos de ventilação, não RPM nem porcentagens de desempenho. O mínimo efetivo é **40% ou o mínimo calibrado, se maior**. Não há parada intencional da fan. A velocidade é interpolada entre os pontos e nunca diminui com o aumento da temperatura. Estes valores são decisões conservadoras do projeto, **não curvas homologadas pela Acer/AMD nem garantia de que a CPU ficará abaixo de uma temperatura específica**.

**Automático Acer** devolve a administração ao firmware, cuja curva não é conhecida. As proteções de curva do A-Fan só atuam quando um preset A-Fan tem controle físico validado. Sem suporte, o aplicativo monitora e avisa; não consegue impor rotação nem temperatura.

Quando o hardware oferece controle validável, o controlador lê a cada segundo, sobe imediatamente e só reduz após 30 segundos com histerese de 3°C, em passos máximos de 5 pontos percentuais por segundo. Todos os presets atingem 100% até 80°C. Ao atingir 85°C, mantém 100% até observar temperatura de no máximo 75°C por 30 segundos; trocar de preset não apaga essa condição. Leituras inválidas, demoradas ou ausentes, RPM zerado e perda do processo independente de recuperação suspendem o controle. Há alerta na bandeja a partir de 85°C, inclusive quando apenas monitora.

A calibração automática começa no máximo, espera CPU abaixo de 60°C e exige retorno ao automático pelo processo independente antes da primeira redução. Depois verifica RPM e testa passos de 10% até 30%, mantendo margem de 10 pontos sobre o mínimo testado. Usa a mediana de três RPM por patamar, exige queda de pelo menos 10% ou 150 RPM (o maior) em um patamar de no máximo 80%, e confirma retorno próximo ao RPM inicial em 100%. Respostas de porcentagem sem mudança física de RPM não bastam. Interrompe se a temperatura superar 65°C durante as reduções, se o RPM zerar ou se os comandos não forem confirmados. A recuperação precisa ser confirmada novamente ao final. Isso verifica resposta de rotação e recuperação; o isolamento em relação à potência da CPU depende de um backend validado.

Se não for possível confirmar o automático após uma falha, o app tenta manter 100% e exibe estado **indeterminado**, sem declarar restauração concluída. Aplicar Automático Acer durante a calibração cancela o procedimento e solicita recuperação.

Uma calibração pertence ao modelo, BIOS e versão do contrato de controle; atualizar a BIOS exige nova calibração. A versão 2 da configuração ignora curvas manuais antigas e converte Personalizado em Auto; os dados de calibração são preservados, mas calibrações do contrato anterior não habilitam controle. O modo automático Acer é diferente de Auto A-Fan. Reduzir refrigeração ainda pode provocar limitação térmica pelo hardware; não há promessa de manter desempenho com qualquer rotação.

## Diagnóstico e desenvolvimento

- `a-fan.exe --diagnose caminho.json`: diagnóstico nativo somente de leitura; execute elevado para acessar a WMI Acer.
- `scripts/diagnose.ps1`: diagnóstico independente em PowerShell, também somente de leitura.
- `a-fan.exe --show`: abrir a janela durante desenvolvimento; a abertura normal permanece oculta.
- `a-fan.exe --exit`: solicitar saída segura de uma instância da mesma sessão.
- `scripts/build.ps1`: formatação, testes, Clippy, release e pacote com SHA-256. Requer Rust e Windows SDK já instalados.

Os relatórios locais não coletam número de série, UUID, credenciais ou dados de outros aplicativos. Configurações têm limite de 16 KB e são gravadas por substituição atômica. Não editar `calibration` manualmente: dados inseridos à mão não substituem validação física.

## Fontes e evidências

- [Linux acer-wmi oficial](https://kernel.googlesource.com/pub/scm/linux/kernel/git/torvalds/linux/+/refs/heads/master/drivers/platform/x86/acer-wmi.c): contrato WMI, status, sensores e controle independente.
- [Microsoft: números WMI](https://learn.microsoft.com/en-us/windows/win32/wmisdk/numbers): uint64 em VARIANT/BSTR e uint32 em VT_I4.
- [AMD Ryzen 7 5700U](https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-5000-series/amd-ryzen-7-5700u.html): Tjmax de 105°C. É uma especificação da CPU, não um alvo recomendado de operação nem uma validação do sensor WMI. Os patamares de 80/85°C são escolhas conservadoras do A-Fan.
- Análise estática com REA/Ghidra 12.1.4 orientou a investigação das consultas de capacidade e estado do agente Acer. Análise estática não comprova execução ou suporte do firmware.
- `scripts/diagnose.ps1` gera `artifacts/diagnostics/hardware.json` localmente. Esse arquivo e os demais diagnósticos não são versionados.

Controles físicos de fan e recuperação em falha não puderam ser ensaiados neste hardware, porque o gate de suporte falhou. Testes de software ou prévias de curva não constituem validação física.
