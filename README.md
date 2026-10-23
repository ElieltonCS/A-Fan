# A-Fan

> **PROJETO EXPERIMENTAL — CONTROLE FÍSICO DE FAN NÃO TESTADO NEM VALIDADO.**
>
> Desenvolvido especificamente para o **Acer Aspire A515-45 com AMD Ryzen 7 5700U, BIOS V1.58 e Windows 11 x64**. Existem testes de software e leituras de sensores, mas a redução real da rotação, a calibração e a recuperação física após falhas **não foram testadas nem validadas neste modelo**. Não há compatibilidade confirmada com outros notebooks. Os perfis de controle permanecem bloqueados quando o firmware não confirma suporte.

Aplicativo Windows x64 nativo em Rust/Win32 para o Acer Aspire A515-45. Abre oculto na bandeja; fechar ou minimizar a janela mantém o monitoramento. O menu **Sair** encerra após restaurar o controle Acer se o aplicativo o tiver alterado.

## Situação neste notebook

Diagnóstico elevado em 23/10/2026: Aspire A515-45, Ryzen 7 5700U, BIOS V1.58. Temperatura CPU e RPM funcionam via WMI. Os getters de modo e porcentagem da ventoinha retornam status **1**, indicando comando indisponível. Por isso, **o A-Fan não altera a fan neste notebook**: o monitoramento e o editor funcionam; aplicar perfis e calibrar permanecem bloqueados. Não foi executado nenhum setter durante a validação desta versão. Isso não demonstra impossibilidade física; demonstra que o backend independente da primeira versão não foi confirmado neste firmware.

O aplicativo não modifica plano de energia, boost, limites CPU/GPU, perfis Acer nem BIOS. Não inclui escrita bruta no EC, drivers de acesso arbitrário a hardware, rede, telemetria externa ou atualização automática. A instalação preserva os componentes Acer existentes.

## Instalar

Este repositório publica o código-fonte; binários, diagnósticos e relatórios da máquina de desenvolvimento ficam fora do Git. Para gerar o pacote, é necessário Windows x64 com Rust estável, ferramentas de compilação C++ e Windows SDK instalados. Na raiz do projeto, execute no PowerShell:

```powershell
.\scripts\build.ps1
```

Depois, na pasta gerada `artifacts/release`, execute `install.ps1` com PowerShell. A instalação solicita administrador, copia o app para `%ProgramFiles%\A-Fan`, cria atalhos e registra a tarefa `A-Fan` para seu logon. Funciona na bateria e na tomada. O app aparece inicialmente só na bandeja; clique no ícone para abrir.

Na janela, **Iniciar com o Windows** habilita/desabilita somente o gatilho de logon. O atalho continua funcionando manualmente. A elevação é feita pela tarefa registrada, sem UAC a cada logon. O executável e os scripts da tarefa ficam em Program Files, protegidos contra edição por processos sem elevação.

Para remover, execute `%ProgramFiles%\A-Fan\uninstall.ps1`. Ele solicita saída segura, remove sua tarefa, seus atalhos e os arquivos conhecidos do app. Preserva as configurações e os diagnósticos em `%LOCALAPPDATA%\A-Fan`.

## Perfis e curva

Perfis prontos: Auto, Silencioso, Resfriar. Personalizado permite editar de 2 a 8 pontos e visualizar o gráfico. **Salvar curva** grava a curva sem mudar o controle ativo. **Aplicar perfil** exige sensores, controle independente, processo de recuperação e calibração válidos. O último ponto é fixo em 85°C/100%; a velocidade não pode diminuir quando a temperatura cresce.

Quando o hardware oferece controle validável, o controlador lê a cada segundo, interpola a curva, sobe rapidamente e só reduz após 30 segundos com histerese de 3°C, em passos máximos de 5 pontos percentuais por segundo. A calibração começa no máximo, espera CPU abaixo de 60°C e exige retorno ao automático pelo processo independente antes da primeira redução. Depois verifica RPM e testa passos de 10% até 30%, mantendo margem de 10 pontos sobre o mínimo testado. Interrompe se a temperatura superar 65°C durante as reduções, se o RPM zerar ou se os comandos não forem confirmados. A recuperação precisa ser confirmada novamente ao final. Se não for possível confirmar o automático após uma falha, o app tenta manter 100% e informa a falha, sem declarar restauração concluída.

Uma calibração pertence ao modelo, BIOS e versão do contrato de controle; atualizar a BIOS exige nova calibração. O modo automático Acer é diferente de Auto A-Fan. Reduzir refrigeração ainda pode provocar limitação térmica pelo hardware; não há promessa de manter desempenho com qualquer rotação.

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
- [AMD Ryzen 7 5700U](https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-5000-series/amd-ryzen-7-5700u.html): especificações e Tjmax. A guarda de 85°C é uma escolha conservadora do A-Fan.
- Análise estática com REA/Ghidra 12.1.4 orientou a investigação das consultas de capacidade e estado do agente Acer. Análise estática não comprova execução ou suporte do firmware.
- `scripts/diagnose.ps1` gera `artifacts/diagnostics/hardware.json` localmente. Esse arquivo e os demais diagnósticos não são versionados.

Controles físicos de fan e recuperação em falha não puderam ser ensaiados neste hardware, porque o gate de suporte falhou. Testes de software ou prévias de curva não constituem validação física.
